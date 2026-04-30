//! Plugin registry index (`index.json`) types and canonical serialization.

use crate::{Dependencies, Description, PluginName, SchemaError, TriggerType};
use serde::Serialize as _;
use serde::ser::Error as _;
use std::fmt;
use std::str::FromStr;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use unicode_normalization::UnicodeNormalization;

/// The `index_schema_version` top-level field. Mirrors `ManifestSchemaVersion`:
/// format `<major>.<minor>`, unsupported majors rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexSchemaVersion {
    major: u32,
    minor: u32,
}

impl IndexSchemaVersion {
    pub const CURRENT_MAJOR: u32 = 2;
    pub const CURRENT_MINOR: u32 = 0;
    pub const CURRENT: Self = Self {
        major: Self::CURRENT_MAJOR,
        minor: Self::CURRENT_MINOR,
    };

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
        if major != Self::CURRENT_MAJOR {
            return Err(SchemaError::UnsupportedIndexMajor {
                found: s.to_owned(),
                supported: Self::CURRENT_MAJOR,
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

/// Registry artifact-base URL. Scheme is restricted to `https`, `http`, or
/// `file`.
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

/// Publication timestamp for a plugin version.
///
/// The wire format mirrors Cargo registry-index `pubtime` exactly:
/// `YYYY-MM-DDTHH:MM:SSZ`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublishedAt(String);

impl PublishedAt {
    const LEN: usize = "YYYY-MM-DDTHH:MM:SSZ".len();

    pub fn try_new(raw: &str) -> Result<Self, SchemaError> {
        if !has_cargo_pubtime_shape(raw) {
            return Err(SchemaError::InvalidPublishedAt {
                value: raw.to_owned(),
            });
        }

        let year = raw[0..4].parse::<i32>().expect("shape checked digits");
        let month = raw[5..7].parse::<u8>().expect("shape checked digits");
        let day = raw[8..10].parse::<u8>().expect("shape checked digits");
        let hour = raw[11..13].parse::<u8>().expect("shape checked digits");
        let minute = raw[14..16].parse::<u8>().expect("shape checked digits");
        let second = raw[17..19].parse::<u8>().expect("shape checked digits");

        let month = Month::try_from(month).map_err(|_| SchemaError::InvalidPublishedAt {
            value: raw.to_owned(),
        })?;
        let date = Date::from_calendar_date(year, month, day).map_err(|_| {
            SchemaError::InvalidPublishedAt {
                value: raw.to_owned(),
            }
        })?;
        let time =
            Time::from_hms(hour, minute, second).map_err(|_| SchemaError::InvalidPublishedAt {
                value: raw.to_owned(),
            })?;
        let _ = PrimitiveDateTime::new(date, time).assume_utc();

        Ok(Self(raw.to_owned()))
    }

    pub fn now_utc() -> Self {
        Self::from_utc_datetime(OffsetDateTime::now_utc())
            .expect("current UTC timestamp must fit Cargo pubtime format")
    }

    pub fn from_utc_datetime(timestamp: OffsetDateTime) -> Result<Self, SchemaError> {
        let timestamp = timestamp.to_offset(UtcOffset::UTC);
        let year = timestamp.year();
        if !(0..=9999).contains(&year) {
            return Err(SchemaError::InvalidPublishedAt {
                value: year.to_string(),
            });
        }

        let value = format!(
            "{year:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            u8::from(timestamp.month()),
            timestamp.day(),
            timestamp.hour(),
            timestamp.minute(),
            timestamp.second()
        );
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn has_cargo_pubtime_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == PublishedAt::LEN
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| matches!(idx, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit())
}

impl fmt::Display for PublishedAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PublishedAt {
    type Err = SchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl<'de> serde::Deserialize<'de> for PublishedAt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for PublishedAt {
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
    pub published_at: PublishedAt,
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

impl IndexEntry {
    pub fn from_manifest(manifest: crate::Manifest, hash: ArtifactHash) -> Self {
        Self::from_manifest_with_published_at(manifest, hash, PublishedAt::now_utc())
    }

    pub fn from_manifest_with_published_at(
        manifest: crate::Manifest,
        hash: ArtifactHash,
        published_at: PublishedAt,
    ) -> Self {
        let plugin = manifest.plugin;
        Self {
            name: plugin.name,
            version: plugin.version,
            published_at,
            description: plugin.description,
            triggers: plugin.triggers,
            homepage: plugin.homepage,
            repository: plugin.repository,
            documentation: plugin.documentation,
            dependencies: manifest.dependencies,
            hash,
            yanked: false,
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Index {
    /// Parses an index from JSON, collecting every field-level defect from
    /// every entry in one pass (including duplicate `(name, version)` pairs).
    ///
    /// Syntax errors and an unsupported/malformed `index_schema_version`
    /// short-circuit with a single error.
    ///
    /// # Examples
    ///
    /// ```
    /// use influxdb3_plugin_schemas::Index;
    ///
    /// let source = r#"{
    ///   "index_schema_version": "2.0",
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
        use std::collections::HashMap;
        use std::str::FromStr;

        let raw: RawIndex = serde_json::from_str(input)
            .map_err(|source| SchemaErrors::single_at_root(SchemaError::JsonParse { source }))?;

        let schema_version =
            IndexSchemaVersion::from_str(&raw.index_schema_version).map_err(|e| {
                SchemaErrors::new(vec![ReportedError::new(
                    FieldPath::root().field("index_schema_version"),
                    e,
                )])
            })?;

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

        // Canonical-keyed index of prior entries: lets us distinguish exact
        // (spelling, version) duplicates (DuplicateIndexEntry) from
        // different-spelling canonical collisions (CanonicalCollision) while
        // reporting accurate payloads. Canonical form folds case and `-`/`_`
        // per Cargo's canon_crate_name.
        let mut seen_by_canonical: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for (i, raw_entry) in raw.plugins.iter().enumerate() {
            let entry_path = root.field("plugins").index(i);
            let validated = validate_raw_entry(raw_entry, &entry_path, &mut errors);

            let canonical = crate::identity::canonical_name(&raw_entry.name);
            let existing = seen_by_canonical.entry(canonical.clone()).or_default();

            let exact_dup = existing
                .iter()
                .any(|(n, v)| n == &raw_entry.name && v == &raw_entry.version);
            if exact_dup {
                errors.push(ReportedError::new(
                    entry_path.clone(),
                    SchemaError::DuplicateIndexEntry {
                        name: raw_entry.name.clone(),
                        version: raw_entry.version.clone(),
                    },
                ));
            } else if existing.iter().any(|(n, _)| n != &raw_entry.name) {
                // Different spelling, canonical-equal → CanonicalCollision.
                // Same spelling + new version is allowed (a new release of
                // the same plugin) and falls through to the append below.
                errors.push(ReportedError::new(
                    entry_path.clone(),
                    SchemaError::CanonicalCollision {
                        name: raw_entry.name.clone(),
                        canonical: canonical.clone(),
                        existing: existing.clone(),
                    },
                ));
            }

            existing.push((raw_entry.name.clone(), raw_entry.version.clone()));

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

    /// Serializes this index to its canonical JSON form:
    ///
    /// - Field ordering matches struct declaration order.
    /// - `plugins[]` sorted by `name` ascending, then `version` ascending by
    ///   SemVer precedence. Metadata fields such as `published_at` and yank
    ///   status do not affect ordering.
    /// - 2-space indent, pretty-printed, trailing newline.
    /// - NFC Unicode normalization on free-text `description` fields. Other
    ///   string fields are already ASCII-constrained by their validators; URL
    ///   fields normalize via `url::Url` parse.
    pub fn to_canonical_json(&self) -> Result<String, SchemaError> {
        let mut normalized = self.clone();
        // `cmp_precedence` ignores build metadata (SemVer 2.0.0 rule); plain
        // `Version::cmp` would lexically order build strings, violating that.
        normalized.plugins.sort_by(|a, b| {
            a.name
                .as_str()
                .cmp(b.name.as_str())
                .then_with(|| a.version.cmp_precedence(&b.version))
        });
        // NFC may grow the string past the 200-char bound via combining-mark
        // sequences; surface that as a structured error rather than panic.
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

    /// Checks whether `entry` can be inserted into this index without
    /// violating uniqueness or canonical-collision invariants.
    ///
    /// Returns `Ok(())` when the insert would be safe, or an
    /// [`crate::IndexInsertError`] describing the conflict.
    ///
    /// This method does **not** modify the index.
    pub fn check_entry_insert(&self, entry: &IndexEntry) -> Result<(), crate::IndexInsertError> {
        use crate::IndexInsertError;

        let new_canonical = entry.name.canonical();

        let existing_canonical: Vec<(String, semver::Version)> = self
            .plugins
            .iter()
            .filter(|e| e.name.canonical() == new_canonical)
            .map(|e| (e.name.as_str().to_owned(), e.version.clone()))
            .collect();

        let any_spelling_differs = existing_canonical
            .iter()
            .any(|(n, _)| n != entry.name.as_str());
        if any_spelling_differs {
            return Err(IndexInsertError::CanonicalCollision {
                name: entry.name.as_str().to_owned(),
                canonical: new_canonical,
                existing: existing_canonical,
            });
        }

        let same_version_dup = existing_canonical.iter().any(|(_, v)| v == &entry.version);
        if same_version_dup {
            let existing_versions: Vec<semver::Version> =
                existing_canonical.iter().map(|(_, v)| v.clone()).collect();
            return Err(IndexInsertError::Duplicate {
                name: entry.name.as_str().to_owned(),
                version: entry.version.clone(),
                existing_versions,
            });
        }

        Ok(())
    }

    /// Validates `entry` against the current index and, if valid, appends it
    /// to `self.plugins`.
    ///
    /// Returns `Err` without modifying the index when the entry would create a
    /// duplicate `(name, version)` pair or a canonical-name collision.
    pub fn push_entry(&mut self, entry: IndexEntry) -> Result<(), crate::IndexInsertError> {
        self.check_entry_insert(&entry)?;
        self.plugins.push(entry);
        Ok(())
    }
}

fn normalize_nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Validates a `RawIndexEntry`, pushing errors into `errors` with paths
/// relative to `path`. Returns `None` if any error was pushed for this entry.
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

    let published_at = match &raw.published_at {
        Some(serde_json::Value::String(value)) => match PublishedAt::try_new(value) {
            Ok(published_at) => Some(published_at),
            Err(e) => {
                errors.push(ReportedError::new(path.field("published_at"), e));
                None
            }
        },
        Some(value) => {
            errors.push(ReportedError::new(
                path.field("published_at"),
                SchemaError::InvalidPublishedAt {
                    value: value.to_string(),
                },
            ));
            None
        }
        None => {
            errors.push(ReportedError::new(
                path.field("published_at"),
                SchemaError::InvalidPublishedAt {
                    value: "<missing>".to_owned(),
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
        published_at: published_at.unwrap(),
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
        let v: IndexSchemaVersion = "2.0".parse().unwrap();
        assert_eq!(v.major(), 2);
        assert_eq!(v.minor(), 0);
    }

    #[test]
    fn rejects_unsupported_major() {
        let err = "3.0".parse::<IndexSchemaVersion>().unwrap_err();
        assert_matches!(err, SchemaError::UnsupportedIndexMajor { .. });
    }

    #[test]
    fn rejects_malformed() {
        assert_matches!(
            "abc".parse::<IndexSchemaVersion>(),
            Err(SchemaError::MalformedSchemaVersion { .. })
        );
    }

    #[test]
    fn current_uses_declared_major_and_minor_constants() {
        assert_eq!(
            IndexSchemaVersion::CURRENT.major(),
            IndexSchemaVersion::CURRENT_MAJOR
        );
        assert_eq!(
            IndexSchemaVersion::CURRENT.minor(),
            IndexSchemaVersion::CURRENT_MINOR
        );
    }

    #[test]
    fn current_to_string_round_trips() {
        let s = IndexSchemaVersion::CURRENT.to_string();
        let parsed: IndexSchemaVersion = s.parse().unwrap();
        assert_eq!(parsed, IndexSchemaVersion::CURRENT);
    }

    #[test]
    fn current_serializes_as_schema_two_zero() {
        assert_eq!(IndexSchemaVersion::CURRENT.to_string(), "2.0");
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
mod published_at_tests {
    use super::*;
    use assert_matches::assert_matches;
    use rstest::rstest;

    #[test]
    fn valid_cargo_pubtime_format_is_accepted() {
        let published_at = PublishedAt::try_new("2026-04-29T18:45:12Z").unwrap();
        assert_eq!(published_at.as_str(), "2026-04-29T18:45:12Z");
        assert_eq!(published_at.to_string(), "2026-04-29T18:45:12Z");
    }

    #[rstest]
    #[case("2026-04-29T18:45:12.123Z")]
    #[case("2026-04-29T13:45:12-05:00")]
    #[case("2026-04-29T18:45:12+00:00")]
    #[case("2026-4-29T18:45:12Z")]
    #[case("2026-04-29t18:45:12Z")]
    #[case("2026-04-29T18:45:12z")]
    #[case("2026-04-29 18:45:12Z")]
    #[case("2026-02-30T18:45:12Z")]
    #[case("2026-04-29T24:00:00Z")]
    fn invalid_cargo_pubtime_format_is_rejected(#[case] input: &str) {
        assert_matches!(
            PublishedAt::try_new(input),
            Err(SchemaError::InvalidPublishedAt { .. })
        );
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use assert_matches::assert_matches;
    use pretty_assertions::assert_eq;

    const MINIMAL: &str = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    {
      "name": "downsampler",
      "version": "1.2.0",
      "published_at": "2026-04-29T18:45:12Z",
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

    fn minimal_with_published_at() -> String {
        MINIMAL.to_owned()
    }

    fn minimal_without_published_at() -> String {
        MINIMAL.replace(
            r#"      "published_at": "2026-04-29T18:45:12Z",
"#,
            "",
        )
    }

    #[test]
    fn parses_published_at_per_entry() {
        let idx = Index::parse_json(&minimal_with_published_at()).unwrap();
        assert_eq!(idx.plugins[0].published_at.as_str(), "2026-04-29T18:45:12Z");
    }

    #[test]
    fn missing_published_at_rejected_per_entry() {
        let errors = Index::parse_json(&minimal_without_published_at()).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidPublishedAt { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[0].published_at");
    }

    #[test]
    fn non_string_published_at_rejected_per_entry() {
        let src = minimal_with_published_at().replace(
            r#""published_at": "2026-04-29T18:45:12Z""#,
            r#""published_at": 123"#,
        );
        let errors = Index::parse_json(&src).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidPublishedAt { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[0].published_at");
    }

    #[test]
    fn malformed_published_at_rejected_per_entry() {
        let src =
            minimal_with_published_at().replace("2026-04-29T18:45:12Z", "2026-04-29T18:45:12.123Z");
        let errors = Index::parse_json(&src).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidPublishedAt { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[0].published_at");
    }

    #[test]
    fn parses_minimal_index() {
        let idx = Index::parse_json(&minimal_with_published_at()).unwrap();
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
        let src = MINIMAL.replace(r#""hash": "#, r#""yanked": true, "hash": "#);
        let idx = Index::parse_json(&src).unwrap();
        assert!(idx.plugins[0].yanked);
    }

    #[test]
    fn yanked_false_parsed() {
        let src = MINIMAL.replace(r#""hash": "#, r#""yanked": false, "hash": "#);
        let idx = Index::parse_json(&src).unwrap();
        assert!(!idx.plugins[0].yanked);
    }

    #[test]
    fn duplicate_name_version_rejected() {
        let dup = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "x", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
    { "name": "x", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x2", "triggers": ["process_writes"],
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
    fn index_rejects_hyphen_underscore_collision() {
        // `foo-bar` and `foo_bar` share the same canonical form (`foo_bar`);
        // the second entry is rejected even though the raw strings differ.
        let json = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "foo-bar", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
    { "name": "foo_bar", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x2", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111" }
  ]
}"#;
        let errors = Index::parse_json(json).expect_err("should reject canonical collision");
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::CanonicalCollision { ref name, ref canonical, ref existing }
                if name == "foo_bar"
                    && canonical == "foo_bar"
                    && existing == &vec![("foo-bar".to_owned(), "1.0.0".to_owned())]
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[1]");
    }

    #[test]
    fn index_rejects_case_collision() {
        // `MyPlugin` and `myplugin` share canonical form `myplugin`.
        let json = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "MyPlugin", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
    { "name": "myplugin", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x2", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111" }
  ]
}"#;
        let errors = Index::parse_json(json).expect_err("should reject case collision");
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::CanonicalCollision { ref name, ref canonical, ref existing }
                if name == "myplugin"
                    && canonical == "myplugin"
                    && existing == &vec![("MyPlugin".to_owned(), "1.0.0".to_owned())]
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[1]");
    }

    #[test]
    fn index_rejects_three_way_canonical_collision() {
        // Three entries collapse to canonical `foo_bar`; first is accepted,
        // second and third each report with their original spelling.
        let json = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "foo-bar", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
    { "name": "foo_bar", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x2", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111" },
    { "name": "FOO-BAR", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x3", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222" }
  ]
}"#;
        let errors = Index::parse_json(json).expect_err("should reject two collisions");
        let e = errors.errors();
        assert_eq!(
            e.len(),
            2,
            "expected 2 errors, got {}: {:?}",
            e.len(),
            e.iter()
                .map(|r| (r.path.as_str(), &r.error))
                .collect::<Vec<_>>()
        );
        assert_matches!(
            e[0].error,
            SchemaError::CanonicalCollision { ref name, ref canonical, ref existing }
                if name == "foo_bar"
                    && canonical == "foo_bar"
                    && existing == &vec![("foo-bar".to_owned(), "1.0.0".to_owned())]
        );
        assert_eq!(e[0].path.as_str(), "plugins[1]");
        assert_matches!(
            e[1].error,
            SchemaError::CanonicalCollision { ref name, ref canonical, ref existing }
                if name == "FOO-BAR"
                    && canonical == "foo_bar"
                    && existing == &vec![
                        ("foo-bar".to_owned(), "1.0.0".to_owned()),
                        ("foo_bar".to_owned(), "1.0.0".to_owned()),
                    ]
        );
        assert_eq!(e[1].path.as_str(), "plugins[2]");
    }

    #[test]
    fn index_rejects_canonical_collision_across_versions() {
        // Previously allowed; now rejected. Different spellings that canonicalize
        // equal must collide regardless of version.
        let json = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "foo-bar", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "x", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
    { "name": "foo_bar", "version": "1.0.1", "published_at": "2026-04-29T18:45:12Z", "description": "x2", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111" }
  ]
}"#;
        let errors =
            Index::parse_json(json).expect_err("should reject canonical collision across versions");
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::CanonicalCollision { ref name, ref canonical, ref existing }
                if name == "foo_bar"
                    && canonical == "foo_bar"
                    && existing == &vec![("foo-bar".to_owned(), "1.0.0".to_owned())]
        );
        assert_eq!(errors.errors()[0].path.as_str(), "plugins[1]");
    }

    #[test]
    fn ignores_unknown_per_entry_field() {
        let with_unknown =
            MINIMAL.replace(r#""hash": "#, r#""experimental_tag": "beta", "hash": "#);
        assert!(Index::parse_json(&with_unknown).is_ok());
    }

    /// Index-entry validation mirrors manifest validation: same trigger and
    /// URL-scheme rules apply per entry.
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
        let src = MINIMAL.replace(r#""hash": "#, r#""homepage": "ftp://bad/", "hash": "#);
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
        let src = MINIMAL.replace(r#""hash": "#, r#""repository": "git://bad/", "hash": "#);
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
            r#""hash": "#,
            r#""documentation": "s3://bucket/docs", "hash": "#,
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
            r#""hash": "#,
            r#""homepage": "http://example.com", "repository": "https://github.com/x/y", "hash": "#,
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

    /// Per-entry defects and duplicate-(name, version) detection collect in
    /// a single pass.
    #[test]
    fn collects_multiple_entry_defects_and_duplicate() {
        // plugins[0] valid; plugins[1] bad hash; plugins[2] duplicates
        // plugins[0]; plugins[3] description too long. Expect 3 errors.
        let long_desc = "a".repeat(201);
        let json = format!(
            r#"{{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    {{ "name": "alpha", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "ok", "triggers": ["process_writes"],
       "dependencies": {{"database_version":">=3.0.0","python":[]}},
       "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" }},
    {{ "name": "beta",  "version": "2.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "ok", "triggers": ["process_writes"],
       "dependencies": {{"database_version":">=3.0.0","python":[]}},
       "hash": "not-a-hash" }},
    {{ "name": "alpha", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "ok", "triggers": ["process_writes"],
       "dependencies": {{"database_version":">=3.0.0","python":[]}},
       "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111" }},
    {{ "name": "gamma", "version": "3.0.0", "published_at": "2026-04-29T18:45:12Z", "description": "{long_desc}", "triggers": ["process_writes"],
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
            paths.contains(&"plugins[1].hash"),
            "missing hash path: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.starts_with("plugins[2]")),
            "missing duplicate path: {paths:?}"
        );
        assert!(
            paths.contains(&"plugins[3].description"),
            "missing description path: {paths:?}"
        );
    }

    #[test]
    fn collects_published_at_defect_with_other_entry_defects() {
        let json = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "alpha", "version": "1.0.0", "published_at": "2026-04-29T18:45:12.123Z", "description": "ok", "triggers": [],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "not-a-hash" }
  ]
}"#;

        let errors = Index::parse_json(json).expect_err("should fail");
        let e = errors.errors();
        assert_eq!(e.len(), 3);
        assert!(
            e.iter().any(|reported| {
                reported.path.as_str() == "plugins[0].published_at"
                    && matches!(reported.error, SchemaError::InvalidPublishedAt { .. })
            }),
            "missing InvalidPublishedAt at plugins[0].published_at: {e:?}"
        );
        assert!(
            e.iter().any(|reported| {
                reported.path.as_str() == "plugins[0].triggers"
                    && matches!(reported.error, SchemaError::EmptyTriggers)
            }),
            "missing EmptyTriggers at plugins[0].triggers: {e:?}"
        );
        assert!(
            e.iter().any(|reported| {
                reported.path.as_str() == "plugins[0].hash"
                    && matches!(reported.error, SchemaError::InvalidHash { .. })
            }),
            "missing InvalidHash at plugins[0].hash: {e:?}"
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

    #[test]
    fn old_non_empty_index_without_published_at_is_rejected_by_schema_version() {
        let json = r#"{
  "index_schema_version": "1.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    { "name": "alpha", "version": "1.0.0", "description": "ok", "triggers": ["process_writes"],
      "dependencies": {"database_version":">=3.0.0","python":[]},
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000" }
  ]
}"#;

        let errors = Index::parse_json(json).expect_err("old schema should fail");
        assert_eq!(errors.errors().len(), 1);
        assert_eq!(errors.errors()[0].path.as_str(), "index_schema_version");
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::UnsupportedIndexMajor { ref found, supported }
                if found == "1.0" && supported == 2
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
            published_at: PublishedAt::try_new("2026-04-29T18:45:12Z").unwrap(),
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
            index_schema_version: IndexSchemaVersion::CURRENT,
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
            index_schema_version: IndexSchemaVersion::CURRENT,
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
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![],
        };
        let out = idx.to_canonical_json().unwrap();
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn two_space_indent() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
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
            index_schema_version: IndexSchemaVersion::CURRENT,
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
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![make_entry("x", semver::Version::new(1, 0, 0))],
        };
        let out = idx.to_canonical_json().unwrap();
        let name_pos = out.find("\"name\"").unwrap();
        let version_pos = out.find("\"version\"").unwrap();
        let published_at_pos = out.find("\"published_at\"").unwrap();
        let description_pos = out.find("\"description\"").unwrap();
        let triggers_pos = out.find("\"triggers\"").unwrap();
        let dependencies_pos = out.find("\"dependencies\"").unwrap();
        let hash_pos = out.find("\"hash\"").unwrap();
        assert!(name_pos < version_pos);
        assert!(version_pos < published_at_pos);
        assert!(published_at_pos < description_pos);
        assert!(description_pos < triggers_pos);
        assert!(triggers_pos < dependencies_pos);
        assert!(dependencies_pos < hash_pos);
    }

    #[test]
    fn published_at_is_preserved_exactly_in_canonical_json() {
        let mut entry = make_entry("x", semver::Version::new(1, 0, 0));
        entry.published_at = PublishedAt::try_new("2027-01-02T03:04:05Z").unwrap();
        let idx = Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![entry],
        };
        let out = idx.to_canonical_json().unwrap();
        assert!(out.contains(r#""published_at": "2027-01-02T03:04:05Z""#));
    }

    #[test]
    fn published_at_does_not_affect_sort() {
        let mut alpha = make_entry("alpha", semver::Version::new(1, 0, 0));
        alpha.published_at = PublishedAt::try_new("2027-01-02T03:04:05Z").unwrap();
        let mut beta = make_entry("beta", semver::Version::new(1, 0, 0));
        beta.published_at = PublishedAt::try_new("2026-04-29T18:45:12Z").unwrap();
        let idx = Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![beta, alpha],
        };
        let out = idx.to_canonical_json().unwrap();
        let alpha_pos = out.find("\"alpha\"").unwrap();
        let beta_pos = out.find("\"beta\"").unwrap();
        assert!(alpha_pos < beta_pos, "name sort must ignore published_at");
    }

    #[test]
    fn parse_canonical_round_trip_is_idempotent() {
        let idx = Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![make_entry("x", semver::Version::new(1, 0, 0))],
        };
        let first = idx.to_canonical_json().unwrap();
        let reparsed = Index::parse_json(&first).unwrap();
        let second = reparsed.to_canonical_json().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn nfc_normalization_makes_precomposed_equal_decomposed() {
        // A uses precomposed "é" (U+00E9); B uses "e" + combining acute
        // (U+0065 U+0301). NFC collapses both to U+00E9, so canonical output
        // is byte-identical.
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
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![entry_a],
        };
        let idx_b = Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
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
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![
                make_entry("beta", semver::Version::new(1, 0, 0)),
                yanked_alpha,
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

    /// SemVer 2.0.0: a prerelease has lower precedence than the corresponding
    /// release (`1.0.0-alpha < 1.0.0`), so canonical ordering must put it
    /// first — otherwise "latest version" queries would be wrong.
    #[test]
    fn prerelease_sorts_before_release_at_same_major_minor_patch() {
        let prerelease = make_entry("p", "1.0.0-alpha".parse().unwrap());
        let release = make_entry("p", semver::Version::new(1, 0, 0));
        let idx = Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            // Reverse-of-expected order forces the sort.
            plugins: vec![release, prerelease],
        };
        let out = idx.to_canonical_json().unwrap();
        let alpha_pos = out.find(r#""1.0.0-alpha""#).unwrap();
        let release_pos = out.find(r#""1.0.0""#).unwrap();
        assert!(
            alpha_pos < release_pos,
            "prerelease 1.0.0-alpha must sort before release 1.0.0"
        );
    }

    /// SemVer 2.0.0: build metadata is ignored for precedence, so entries
    /// differing only in build metadata must compare equal via
    /// `cmp_precedence`. Plain `Version::cmp` would order them lexically on
    /// the build string, so `to_canonical_json` must use `cmp_precedence`.
    #[test]
    fn build_metadata_does_not_affect_precedence() {
        let v_a: semver::Version = "1.0.0+build.1".parse().unwrap();
        let v_b: semver::Version = "1.0.0+build.2".parse().unwrap();

        assert_eq!(v_a.cmp_precedence(&v_b), std::cmp::Ordering::Equal);
        // Sanity: plain `cmp` distinguishes them, which is why the sort
        // cannot use `cmp` here.
        assert_ne!(v_a.cmp(&v_b), std::cmp::Ordering::Equal);

        let idx = Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            // Equal-precedence entries preserve input order via stable sort.
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

#[cfg(test)]
mod insert_tests {
    use super::*;
    use crate::{
        ArtifactHash, ArtifactsUrl, Dependencies, Description, IndexInsertError, TriggerType,
    };
    use assert_matches::assert_matches;

    fn empty_index() -> Index {
        Index {
            index_schema_version: IndexSchemaVersion::CURRENT,
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins: vec![],
        }
    }

    fn make_entry(name: &str, version: semver::Version) -> IndexEntry {
        IndexEntry {
            name: name.parse().unwrap(),
            version,
            published_at: PublishedAt::try_new("2026-04-29T18:45:12Z").unwrap(),
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
    fn empty_index_accepts_append() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap();
        assert_eq!(idx.plugins.len(), 1);
    }

    #[test]
    fn same_spelling_different_version_accepted() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 1, 0)))
            .unwrap();
        assert_eq!(idx.plugins.len(), 2);
    }

    #[test]
    fn same_spelling_same_version_returns_duplicate() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap();
        let err = idx
            .push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap_err();
        assert_matches!(err, IndexInsertError::Duplicate { .. });
    }

    #[test]
    fn duplicate_error_lists_existing_versions_in_index_order() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 1, 0)))
            .unwrap();
        let err = idx
            .push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap_err();
        match err {
            IndexInsertError::Duplicate {
                existing_versions, ..
            } => {
                assert_eq!(
                    existing_versions,
                    vec![semver::Version::new(1, 0, 0), semver::Version::new(1, 1, 0)]
                );
            }
            other => panic!("expected Duplicate, got {other:?}"),
        }
    }

    #[test]
    fn hyphen_underscore_canonical_collision_rejected() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("foo-bar", semver::Version::new(1, 0, 0)))
            .unwrap();
        let err = idx
            .push_entry(make_entry("foo_bar", semver::Version::new(1, 0, 0)))
            .unwrap_err();
        assert_matches!(err, IndexInsertError::CanonicalCollision { .. });
    }

    #[test]
    fn case_only_canonical_collision_rejected() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("MyPlugin", semver::Version::new(1, 0, 0)))
            .unwrap();
        let err = idx
            .push_entry(make_entry("myplugin", semver::Version::new(1, 0, 0)))
            .unwrap_err();
        assert_matches!(err, IndexInsertError::CanonicalCollision { .. });
    }

    #[test]
    fn canonical_collision_rejected_even_when_version_differs() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("foo-bar", semver::Version::new(1, 0, 0)))
            .unwrap();
        let err = idx
            .push_entry(make_entry("foo_bar", semver::Version::new(2, 0, 0)))
            .unwrap_err();
        assert_matches!(err, IndexInsertError::CanonicalCollision { .. });
    }

    #[test]
    fn index_unchanged_after_failed_push_entry() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap();
        let snapshot = idx.clone();
        let _ = idx.push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)));
        assert_eq!(idx, snapshot);
    }

    #[test]
    fn check_entry_insert_does_not_mutate() {
        let mut idx = empty_index();
        idx.push_entry(make_entry("alpha", semver::Version::new(1, 0, 0)))
            .unwrap();
        let snapshot = idx.clone();
        let entry = make_entry("alpha", semver::Version::new(2, 0, 0));
        idx.check_entry_insert(&entry).unwrap();
        assert_eq!(idx, snapshot);
    }
}

#[cfg(test)]
mod from_manifest_tests {
    use super::*;
    use crate::{
        ArtifactHash, Dependencies, Description, Manifest, ManifestSchemaVersion, PluginMetadata,
        PythonRequirement, TriggerType,
    };

    fn sample_manifest() -> Manifest {
        Manifest {
            manifest_schema_version: ManifestSchemaVersion::new(1, 1),
            plugin: PluginMetadata {
                name: "downsampler".parse().unwrap(),
                version: semver::Version::new(1, 2, 0),
                description: Description::try_new("A downsampling plugin").unwrap(),
                triggers: vec![
                    TriggerType::ProcessWrites,
                    TriggerType::ProcessScheduledCall,
                ],
                homepage: Some(url::Url::parse("https://example.com").unwrap()),
                repository: Some(url::Url::parse("https://github.com/example/repo").unwrap()),
                documentation: Some(url::Url::parse("https://docs.example.com").unwrap()),
            },
            dependencies: Dependencies {
                database_version: ">=3.2.0,<4.0.0".parse().unwrap(),
                python: vec![PythonRequirement::try_new("requests>=2.31,<3").unwrap()],
            },
        }
    }

    fn sample_hash() -> ArtifactHash {
        ArtifactHash::try_new(
            "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        )
        .unwrap()
    }

    #[test]
    fn copies_name() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(entry.name.as_str(), "downsampler");
    }

    #[test]
    fn copies_version() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(entry.version, semver::Version::new(1, 2, 0));
    }

    #[test]
    fn copies_description() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(entry.description.as_str(), "A downsampling plugin");
    }

    #[test]
    fn copies_triggers() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(
            entry.triggers,
            vec![
                TriggerType::ProcessWrites,
                TriggerType::ProcessScheduledCall
            ]
        );
    }

    #[test]
    fn copies_homepage() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(entry.homepage.unwrap().as_str(), "https://example.com/");
    }

    #[test]
    fn copies_repository() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(
            entry.repository.unwrap().as_str(),
            "https://github.com/example/repo"
        );
    }

    #[test]
    fn copies_documentation() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(
            entry.documentation.unwrap().as_str(),
            "https://docs.example.com/"
        );
    }

    #[test]
    fn copies_dependencies() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert_eq!(entry.dependencies.python.len(), 1);
    }

    #[test]
    fn copies_hash() {
        let h = sample_hash();
        let entry = IndexEntry::from_manifest(sample_manifest(), h.clone());
        assert_eq!(entry.hash, h);
    }

    #[test]
    fn copies_injected_published_at() {
        let published_at = PublishedAt::try_new("2027-01-02T03:04:05Z").unwrap();
        let entry = IndexEntry::from_manifest_with_published_at(
            sample_manifest(),
            sample_hash(),
            published_at.clone(),
        );
        assert_eq!(entry.published_at, published_at);
    }

    #[test]
    fn from_manifest_assigns_current_utc_published_at() {
        let before = PublishedAt::now_utc();
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        let after = PublishedAt::now_utc();
        assert!(entry.published_at >= before);
        assert!(entry.published_at <= after);
        assert_eq!(entry.published_at.as_str().len(), PublishedAt::LEN);
        assert!(entry.published_at.as_str().ends_with('Z'));
        assert!(!entry.published_at.as_str().contains('.'));
        assert!(!entry.published_at.as_str().contains('+'));
    }

    #[test]
    fn yanked_is_false() {
        let entry = IndexEntry::from_manifest(sample_manifest(), sample_hash());
        assert!(!entry.yanked);
    }
}

//! Plugin manifest (`manifest.toml`) types and parsing.

use crate::SchemaError;
use std::fmt;
use std::str::FromStr;

/// Supported major version of the manifest schema. Bumped when breaking changes
/// are introduced; consumers refuse to parse unsupported majors per Spec 1.
pub(crate) const SUPPORTED_MANIFEST_MAJOR: u32 = 1;

/// The `manifest_schema_version` top-level field.
///
/// Format: `<major>.<minor>`. Consumers reject unsupported majors per Spec 1's
/// Schema Versioning Strategy. Within a known major, unknown fields are
/// tolerated by the structural parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ManifestSchemaVersion {
    major: u32,
    minor: u32,
}

impl ManifestSchemaVersion {
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

impl fmt::Display for ManifestSchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for ManifestSchemaVersion {
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

        if major != SUPPORTED_MANIFEST_MAJOR {
            return Err(SchemaError::UnsupportedManifestMajor {
                found: s.to_owned(),
                supported: SUPPORTED_MANIFEST_MAJOR,
            });
        }
        Ok(Self { major, minor })
    }
}

impl<'de> serde::Deserialize<'de> for ManifestSchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for ManifestSchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

/// One-line plugin description. 1–200 characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description(String);

impl Description {
    pub fn try_new(s: &str) -> Result<Self, SchemaError> {
        if s.is_empty() {
            return Err(SchemaError::DescriptionEmpty);
        }
        let len = s.chars().count();
        if len > 200 {
            return Err(SchemaError::DescriptionTooLong { len });
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for Description {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for Description {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

/// Closed set of supported trigger types. See Spec 2 Validation: trigger
/// identifiers must be drawn from this set or the manifest is rejected.
///
/// Serde goes through `TryFrom<String>` / `Into<String>` (which delegate to
/// `FromStr` / `as_str`), so a `rename_all` attribute would be a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum TriggerType {
    ProcessWrites,
    ProcessScheduledCall,
    ProcessRequest,
}

impl TriggerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProcessWrites => "process_writes",
            Self::ProcessScheduledCall => "process_scheduled_call",
            Self::ProcessRequest => "process_request",
        }
    }
}

impl fmt::Display for TriggerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TriggerType {
    type Err = SchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "process_writes" => Ok(Self::ProcessWrites),
            "process_scheduled_call" => Ok(Self::ProcessScheduledCall),
            "process_request" => Ok(Self::ProcessRequest),
            other => Err(SchemaError::UnknownTriggerType {
                trigger: other.to_owned(),
            }),
        }
    }
}

impl TryFrom<String> for TriggerType {
    type Error = SchemaError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<TriggerType> for String {
    fn from(value: TriggerType) -> Self {
        value.as_str().to_owned()
    }
}

/// A PEP 508 Python package requirement string (e.g., `requests>=2.31,<3`).
/// Validated for parseability at construction; stored in its canonical string
/// form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonRequirement(String);

impl PythonRequirement {
    pub fn try_new(s: &str) -> Result<Self, SchemaError> {
        // Parse for validation only; we store the original string.
        //
        // API note: `pep508_rs = "0.9"` has `Requirement` generic over URL type.
        // If the installed version exposes `Requirement::from_str` without a
        // type parameter, drop the turbofish. If `Pep508Error`'s path is
        // different, update `SchemaError::InvalidPythonRequirement`'s `source`
        // field type in `error.rs` to match.
        pep508_rs::Requirement::<pep508_rs::VerbatimUrl>::from_str(s).map_err(|e| {
            SchemaError::InvalidPythonRequirement {
                requirement: s.to_owned(),
                source: Box::new(e),
            }
        })?;
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for PythonRequirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for PythonRequirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

/// A parsed plugin manifest.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Manifest {
    pub manifest_schema_version: ManifestSchemaVersion,
    pub plugin: PluginMetadata,
    pub dependencies: Dependencies,
}

impl Manifest {
    /// Parses a manifest from a TOML string.
    ///
    /// Structural parsing via serde + newtype `Deserialize` impls runs first;
    /// post-parse validation for rules serde can't express (non-empty triggers,
    /// URL scheme allowlist) runs after.
    ///
    /// # Examples
    ///
    /// ```
    /// use influxdb3_plugin_schemas::Manifest;
    ///
    /// let source = r#"
    /// manifest_schema_version = "1.0"
    ///
    /// [plugin]
    /// name = "example"
    /// version = "0.1.0"
    /// description = "Example plugin."
    /// triggers = ["process_writes"]
    ///
    /// [dependencies]
    /// database_version = ">=3.0.0"
    /// "#;
    ///
    /// let manifest = Manifest::parse_toml(source).unwrap();
    /// assert_eq!(manifest.plugin.name.as_str(), "example");
    /// ```
    pub fn parse_toml(input: &str) -> Result<Self, SchemaError> {
        let parsed: Self =
            toml::from_str(input).map_err(|source| SchemaError::TomlParse { source })?;
        parsed.validate()?;
        Ok(parsed)
    }

    fn validate(&self) -> Result<(), SchemaError> {
        if self.plugin.triggers.is_empty() {
            return Err(SchemaError::EmptyTriggers);
        }
        for url in [
            &self.plugin.homepage,
            &self.plugin.repository,
            &self.plugin.documentation,
        ]
        .into_iter()
        .flatten()
        {
            let scheme = url.scheme();
            if !matches!(scheme, "http" | "https") {
                return Err(SchemaError::InvalidUrlScheme {
                    url: url.to_string(),
                    scheme: scheme.to_owned(),
                });
            }
        }
        Ok(())
    }
}

// Note: no `to_toml_string` method in Plan 1. Manifests are author-written TOML
// files; the SDK parses them, never emits them. If a future plan needs TOML
// serialization (e.g., scaffolding commands in the CLI), add it then with a
// dedicated `SchemaError::TomlSerialize { source: toml::ser::Error }` variant
// — do not cast serialize errors through `toml::de::Error::custom`.

/// `[plugin]` section of the manifest.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PluginMetadata {
    pub name: crate::PluginName,
    pub version: semver::Version,
    pub description: Description,
    pub triggers: Vec<TriggerType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<url::Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<url::Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<url::Url>,
}

/// `[dependencies]` section of the manifest.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Dependencies {
    pub database_version: semver::VersionReq,
    #[serde(default)]
    pub python: Vec<PythonRequirement>,
}

#[cfg(test)]
mod schema_version_tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn parses_major_minor() {
        let v: ManifestSchemaVersion = "1.0".parse().unwrap();
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 0);
    }

    #[test]
    fn parses_higher_minor_within_known_major() {
        let v: ManifestSchemaVersion = "1.42".parse().unwrap();
        assert_eq!((v.major(), v.minor()), (1, 42));
    }

    #[test]
    fn rejects_malformed() {
        assert_matches!(
            "1".parse::<ManifestSchemaVersion>(),
            Err(SchemaError::MalformedSchemaVersion { .. })
        );
        assert_matches!(
            "1.0.0".parse::<ManifestSchemaVersion>(),
            Err(SchemaError::MalformedSchemaVersion { .. })
        );
        assert_matches!(
            "a.b".parse::<ManifestSchemaVersion>(),
            Err(SchemaError::MalformedSchemaVersion { .. })
        );
    }

    #[test]
    fn rejects_unsupported_major() {
        let err = "2.0".parse::<ManifestSchemaVersion>().unwrap_err();
        assert_matches!(err, SchemaError::UnsupportedManifestMajor { .. });
    }

    #[test]
    fn display_round_trip() {
        let v = ManifestSchemaVersion::new(1, 3);
        assert_eq!(format!("{v}"), "1.3");
        let parsed: ManifestSchemaVersion = "1.3".parse().unwrap();
        assert_eq!(parsed, v);
    }
}

#[cfg(test)]
mod description_tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn accepts_up_to_200_chars() {
        let ok_200 = "a".repeat(200);
        let d = Description::try_new(&ok_200).unwrap();
        assert_eq!(d.as_str().chars().count(), 200);
    }

    #[test]
    fn rejects_201_chars() {
        let too_long = "a".repeat(201);
        assert_matches!(
            Description::try_new(&too_long),
            Err(SchemaError::DescriptionTooLong { len: 201 })
        );
    }

    #[test]
    fn rejects_empty() {
        assert_matches!(Description::try_new(""), Err(SchemaError::DescriptionEmpty));
    }

    #[test]
    fn accepts_single_char() {
        assert!(Description::try_new("x").is_ok());
    }
}

#[cfg(test)]
mod trigger_type_tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("process_writes", TriggerType::ProcessWrites)]
    #[case("process_scheduled_call", TriggerType::ProcessScheduledCall)]
    #[case("process_request", TriggerType::ProcessRequest)]
    fn valid_triggers_parse(#[case] input: &str, #[case] expected: TriggerType) {
        assert_eq!(input.parse::<TriggerType>().unwrap(), expected);
    }

    #[rstest]
    #[case("on_startup")]
    #[case("process_Writes")] // case sensitive
    #[case("")]
    fn invalid_triggers_rejected(#[case] input: &str) {
        assert!(input.parse::<TriggerType>().is_err());
    }

    #[test]
    fn serde_round_trip() {
        let t = TriggerType::ProcessScheduledCall;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"process_scheduled_call\"");
        let back: TriggerType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn serde_rejects_unknown() {
        let result: Result<TriggerType, _> = serde_json::from_str("\"on_startup\"");
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod python_requirement_tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn accepts_simple_requirement() {
        assert!(PythonRequirement::try_new("requests>=2.31,<3").is_ok());
    }

    #[test]
    fn accepts_compatible_release() {
        assert!(PythonRequirement::try_new("pydantic~=2.0").is_ok());
    }

    #[test]
    fn rejects_malformed() {
        // Double-operator `>>=` is unambiguously rejected by PEP 508.
        assert_matches!(
            PythonRequirement::try_new("requests>>=2.0"),
            Err(SchemaError::InvalidPythonRequirement { .. })
        );
    }

    #[test]
    fn preserves_original_string() {
        let r = PythonRequirement::try_new("requests>=2.31,<3").unwrap();
        assert_eq!(r.as_str(), "requests>=2.31,<3");
    }
}

#[cfg(test)]
mod manifest_parse_tests {
    use super::*;
    use assert_matches::assert_matches;
    use pretty_assertions::assert_eq;

    const MINIMAL: &str = r#"
manifest_schema_version = "1.0"

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Test plugin"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.2.0,<4.0.0"
"#;

    const FULL: &str = r#"
manifest_schema_version = "1.0"

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Notify an HTTP endpoint on every WAL commit."
triggers = ["process_writes", "process_scheduled_call"]
homepage = "https://influxdata.com"
repository = "https://github.com/influxdata/plugin-downsampler"
documentation = "https://github.com/influxdata/plugin-downsampler/readme.md"

[dependencies]
database_version = ">=3.2.0,<4.0.0"
python = ["requests>=2.31,<3", "pydantic~=2.0"]
"#;

    #[test]
    fn parses_minimal_manifest() {
        let m = Manifest::parse_toml(MINIMAL).expect("minimal manifest should parse");
        assert_eq!(m.plugin.name.as_str(), "downsampler");
        assert_eq!(m.plugin.version, semver::Version::new(1, 2, 0));
        assert_eq!(m.plugin.triggers.len(), 1);
    }

    #[test]
    fn parses_full_manifest() {
        let m = Manifest::parse_toml(FULL).expect("full manifest should parse");
        assert_eq!(m.plugin.triggers.len(), 2);
        assert_eq!(m.dependencies.python.len(), 2);
        assert!(m.plugin.homepage.is_some());
    }

    #[test]
    fn parses_snapshot_matches() {
        let m = Manifest::parse_toml(FULL).unwrap();
        insta::assert_debug_snapshot!("full_manifest_parsed", m);
    }

    #[test]
    fn rejects_missing_plugin_section() {
        let missing = r#"
manifest_schema_version = "1.0"

[dependencies]
database_version = ">=3.2.0"
"#;
        let err = Manifest::parse_toml(missing).unwrap_err();
        assert_matches!(err, SchemaError::TomlParse { .. });
    }

    #[test]
    fn rejects_missing_schema_version() {
        let missing = r#"
[plugin]
name = "x"
version = "1.0.0"
description = "x"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.2.0"
"#;
        let err = Manifest::parse_toml(missing).unwrap_err();
        assert_matches!(err, SchemaError::TomlParse { .. });
    }

    #[test]
    fn ignores_unknown_top_level_field() {
        // Appending to MINIMAL would place the field inside `[dependencies]`
        // (TOML treats subsequent key-value pairs as belonging to the most
        // recently opened table). Build a fresh document with the unknown
        // field above any table header so it's unambiguously top-level.
        let with_unknown = r#"
manifest_schema_version = "1.0"
experimental_feature = true

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Test plugin"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.2.0,<4.0.0"
"#;
        assert!(Manifest::parse_toml(with_unknown).is_ok());
    }

    #[test]
    fn parses_one_one_schema_version() {
        // Spec 1's manifest example uses "1.1" — verify it parses under the
        // current supported major (1).
        let src = MINIMAL.replace(
            r#"manifest_schema_version = "1.0""#,
            r#"manifest_schema_version = "1.1""#,
        );
        let m = Manifest::parse_toml(&src).unwrap();
        assert_eq!(m.manifest_schema_version.minor(), 1);
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use assert_matches::assert_matches;
    use rstest::rstest;

    fn with_fragment(key: &str, value: &str) -> String {
        format!(
            r#"
manifest_schema_version = "1.0"

[plugin]
name = "x"
version = "1.0.0"
description = "x"
triggers = ["process_writes"]
{key} = {value}

[dependencies]
database_version = ">=3.0.0"
"#
        )
    }

    #[rstest]
    #[case("homepage", r#""ftp://bad/""#)]
    #[case("homepage", r#""file:///local""#)]
    #[case("repository", r#""git://bad""#)]
    #[case("documentation", r#""s3://bucket""#)]
    fn rejects_non_http_urls(#[case] field: &str, #[case] value: &str) {
        let manifest = with_fragment(field, value);
        let err = Manifest::parse_toml(&manifest).unwrap_err();
        assert_matches!(err, SchemaError::InvalidUrlScheme { .. });
    }

    #[rstest]
    #[case("homepage", r#""http://example.com""#)]
    #[case("homepage", r#""https://example.com""#)]
    #[case("repository", r#""https://github.com/foo/bar""#)]
    #[case("documentation", r#""http://docs.example.com/plugin""#)]
    fn accepts_http_and_https_urls(#[case] field: &str, #[case] value: &str) {
        let manifest = with_fragment(field, value);
        Manifest::parse_toml(&manifest)
            .unwrap_or_else(|e| panic!("expected {field}={value} to parse, got {e}"));
    }

    #[test]
    fn rejects_empty_triggers() {
        let input = r#"
manifest_schema_version = "1.0"

[plugin]
name = "x"
version = "1.0.0"
description = "x"
triggers = []

[dependencies]
database_version = ">=3.0.0"
"#;
        let err = Manifest::parse_toml(input).unwrap_err();
        assert_matches!(err, SchemaError::EmptyTriggers);
    }
}

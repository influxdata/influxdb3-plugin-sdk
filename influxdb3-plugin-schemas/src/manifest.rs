//! Plugin manifest (`manifest.toml`) types and parsing.

use crate::{PluginName, SchemaError};
use std::fmt;
use std::str::FromStr;

/// Supported major. Parsers refuse unsupported majors; bumped on breaking
/// schema changes.
pub(crate) const SUPPORTED_MANIFEST_MAJOR: u32 = 1;

/// The `manifest_schema_version` top-level field, format `<major>.<minor>`.
///
/// Unsupported majors are rejected. Within a known major, unknown fields are
/// tolerated by the structural parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ManifestSchemaVersion {
    major: u32,
    minor: u32,
}

impl ManifestSchemaVersion {
    pub const CURRENT: Self = Self { major: 1, minor: 2 };

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
        // The newline check is the more specific rule, so it precedes the
        // length check: a 201-char string that also contains a newline is
        // reported as multiline rather than too-long.
        if s.contains('\n') || s.contains('\r') {
            return Err(SchemaError::DescriptionMultiline {
                len: s.chars().count(),
            });
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

/// Closed set of supported trigger types. Manifests are rejected if any
/// trigger identifier is outside this set.
///
/// Serde goes through `TryFrom<String>` / `Into<String>`, so `rename_all`
/// would be a no-op.
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
        // Parse for validation only; store the original string. The
        // `<VerbatimUrl>` turbofish tracks pep508_rs's pre-1.0 generic
        // Requirement; on upgrade, also review SchemaError::InvalidPythonRequirement.
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
    /// Parses a manifest from TOML, reporting every field-level defect in one
    /// pass via `SchemaErrors`.
    ///
    /// # Errors
    ///
    /// Returns `Err(SchemaErrors)` with a single `TomlParse` error if TOML
    /// syntax fails; a single error if `manifest_schema_version` is malformed
    /// or unsupported (short-circuit, no field-level validation); or one or
    /// more field-level errors with field-path context.
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
    pub fn parse_toml(input: &str) -> Result<Self, crate::SchemaErrors> {
        use crate::raw::RawManifest;
        use crate::{FieldPath, ReportedError, SchemaErrors};
        use std::str::FromStr;

        // Phase 1: raw deserialize. Syntax / required-field errors are fatal.
        let raw: RawManifest = toml::from_str(input)
            .map_err(|source| SchemaErrors::single_at_root(SchemaError::TomlParse { source }))?;

        // Phase 2a: schema-version short-circuit — skips field-level validation.
        let schema_version = ManifestSchemaVersion::from_str(&raw.manifest_schema_version)
            .map_err(|e| {
                SchemaErrors::new(vec![ReportedError::new(
                    FieldPath::root().field("manifest_schema_version"),
                    e,
                )])
            })?;

        // Phase 2b: collect field-level errors.
        let mut errors = Vec::new();
        let plugin_path = FieldPath::root().field("plugin");
        let deps_path = FieldPath::root().field("dependencies");

        let name = PluginName::from_str(&raw.plugin.name);
        let name_ok = name.as_ref().ok().cloned();
        if let Err(e) = name {
            errors.push(ReportedError::new(plugin_path.field("name"), e));
        }

        let version = semver::Version::parse(&raw.plugin.version).map_err(|source| {
            SchemaError::InvalidVersion {
                version: raw.plugin.version.clone(),
                source,
            }
        });
        let version_ok = version.as_ref().ok().cloned();
        if let Err(e) = version {
            errors.push(ReportedError::new(plugin_path.field("version"), e));
        }

        let description = Description::try_new(&raw.plugin.description);
        let description_ok = description.as_ref().ok().cloned();
        if let Err(e) = description {
            errors.push(ReportedError::new(plugin_path.field("description"), e));
        }

        // Triggers: non-empty + each entry must parse as TriggerType.
        let mut triggers_ok: Vec<TriggerType> = Vec::with_capacity(raw.plugin.triggers.len());
        if raw.plugin.triggers.is_empty() {
            errors.push(ReportedError::new(
                plugin_path.field("triggers"),
                SchemaError::EmptyTriggers,
            ));
        } else {
            for (i, trig) in raw.plugin.triggers.iter().enumerate() {
                match TriggerType::from_str(trig) {
                    Ok(t) => triggers_ok.push(t),
                    Err(e) => errors.push(ReportedError::new(
                        plugin_path.field("triggers").index(i),
                        e,
                    )),
                }
            }
        }

        // Optional URL fields: must parse and use http/https scheme when present.
        let homepage = parse_optional_http_url_from_path(
            &raw.plugin.homepage,
            &mut errors,
            &plugin_path,
            "homepage",
        );
        let repository = parse_optional_http_url_from_path(
            &raw.plugin.repository,
            &mut errors,
            &plugin_path,
            "repository",
        );
        let documentation = parse_optional_http_url_from_path(
            &raw.plugin.documentation,
            &mut errors,
            &plugin_path,
            "documentation",
        );

        let database_version = semver::VersionReq::parse(&raw.dependencies.database_version)
            .map_err(|source| SchemaError::InvalidDatabaseVersion {
                range: raw.dependencies.database_version.clone(),
                source,
            });
        let database_version_ok = database_version.as_ref().ok().cloned();
        if let Err(e) = database_version {
            errors.push(ReportedError::new(deps_path.field("database_version"), e));
        }

        let mut python_ok: Vec<PythonRequirement> =
            Vec::with_capacity(raw.dependencies.python.len());
        for (i, p) in raw.dependencies.python.iter().enumerate() {
            match PythonRequirement::try_new(p) {
                Ok(pr) => python_ok.push(pr),
                Err(e) => errors.push(ReportedError::new(deps_path.field("python").index(i), e)),
            }
        }

        if !errors.is_empty() {
            return Err(SchemaErrors::new(errors));
        }

        // Safe unwraps: each `_ok` is `Some(_)` whenever no error was pushed.
        Ok(Manifest {
            manifest_schema_version: schema_version,
            plugin: PluginMetadata {
                name: name_ok.unwrap(),
                version: version_ok.unwrap(),
                description: description_ok.unwrap(),
                triggers: triggers_ok,
                homepage,
                repository,
                documentation,
                exclude: raw.plugin.exclude,
            },
            dependencies: Dependencies {
                database_version: database_version_ok.unwrap(),
                python: python_ok,
            },
        })
    }
}

/// Parses an optional URL field, requiring `http` or `https` scheme. Returns
/// `None` when absent; on parse or scheme failure, pushes a `ReportedError`
/// and returns `None`. Shared with `index.rs` for per-entry URL validation.
pub(crate) fn parse_optional_http_url_from_path(
    raw: &Option<String>,
    errors: &mut Vec<crate::ReportedError>,
    parent: &crate::FieldPath,
    field_name: &str,
) -> Option<url::Url> {
    use crate::ReportedError;

    let raw = raw.as_deref()?;
    match url::Url::parse(raw) {
        Ok(u) => match u.scheme() {
            "http" | "https" => Some(u),
            other => {
                errors.push(ReportedError::new(
                    parent.field(field_name),
                    SchemaError::InvalidUrlScheme {
                        url: raw.to_owned(),
                        scheme: other.to_owned(),
                    },
                ));
                None
            }
        },
        Err(source) => {
            errors.push(ReportedError::new(
                parent.field(field_name),
                SchemaError::InvalidUrl {
                    url: raw.to_owned(),
                    source,
                },
            ));
            None
        }
    }
}

// No TOML serializer: manifests are author-written and the SDK never emits
// them. If one is added later, introduce a dedicated
// `SchemaError::TomlSerialize { source: toml::ser::Error }` variant rather
// than casting through `toml::de::Error::custom`.

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
    /// Gitignore-style patterns, relative to the plugin root, naming files to
    /// omit from source-file selection (packaging + validation). Optional;
    /// missing or `[]` means no manifest-level exclusions. Pattern *syntax* is
    /// validated by the SDK at selection time, not here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
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

    #[test]
    fn current_major_equals_supported() {
        assert_eq!(
            ManifestSchemaVersion::CURRENT.major(),
            SUPPORTED_MANIFEST_MAJOR
        );
    }

    #[test]
    fn current_to_string_round_trips() {
        let s = ManifestSchemaVersion::CURRENT.to_string();
        let parsed: ManifestSchemaVersion = s.parse().unwrap();
        assert_eq!(parsed, ManifestSchemaVersion::CURRENT);
    }

    #[test]
    fn current_is_one_two() {
        assert_eq!(
            (ManifestSchemaVersion::CURRENT.major(), ManifestSchemaVersion::CURRENT.minor()),
            (1, 2)
        );
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

    #[test]
    fn rejects_multiline_description_lf() {
        assert_matches!(
            Description::try_new("first\nsecond"),
            Err(SchemaError::DescriptionMultiline { .. })
        );
    }

    #[test]
    fn rejects_multiline_description_crlf() {
        assert_matches!(
            Description::try_new("first\r\nsecond"),
            Err(SchemaError::DescriptionMultiline { .. })
        );
    }

    #[test]
    fn rejects_multiline_description_cr() {
        assert_matches!(
            Description::try_new("first\rsecond"),
            Err(SchemaError::DescriptionMultiline { .. })
        );
    }

    /// A 201-char string containing a newline must be reported as multiline,
    /// not as too-long. The newline rule is the more specific.
    /// `rejects_201_chars` proves that the same 201-char input absent a
    /// newline fires `DescriptionTooLong`; together they pin precedence.
    #[test]
    fn multiline_check_precedes_length_check() {
        let s = format!("{}\n{}", "a".repeat(100), "b".repeat(100));
        assert_eq!(s.chars().count(), 201, "fixture sanity: input is 201 chars");
        let err = Description::try_new(&s).expect_err("must reject");
        let SchemaError::DescriptionMultiline { len } = err else {
            panic!("expected DescriptionMultiline, got {err:?}");
        };
        assert_eq!(len, 201);
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
    #[case("process_Writes")]
    #[case("")]
    fn invalid_triggers_rejected(#[case] input: &str) {
        use assert_matches::assert_matches;
        assert_matches!(
            input.parse::<TriggerType>(),
            Err(SchemaError::UnknownTriggerType { .. })
        );
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
        let err = result.expect_err("should reject unknown trigger");
        assert!(
            err.to_string().contains("on_startup"),
            "error should name the rejected trigger, got: {err}"
        );
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
        // `>>=` (double operator) is unambiguously rejected by PEP 508.
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
        let errors = Manifest::parse_toml(missing).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_eq!(errors.errors()[0].path.as_str(), "");
        assert_matches!(errors.errors()[0].error, SchemaError::TomlParse { .. });
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
        let errors = Manifest::parse_toml(missing).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_eq!(errors.errors()[0].path.as_str(), "");
        assert_matches!(errors.errors()[0].error, SchemaError::TomlParse { .. });
    }

    #[test]
    fn ignores_unknown_top_level_field() {
        // Field is placed above any table header so it's unambiguously
        // top-level (appending to MINIMAL would land it in `[dependencies]`).
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
        let src = MINIMAL.replace(
            r#"manifest_schema_version = "1.0""#,
            r#"manifest_schema_version = "1.1""#,
        );
        let m = Manifest::parse_toml(&src).unwrap();
        assert_eq!(m.manifest_schema_version.minor(), 1);
    }

    /// N distinct field-level defects must produce exactly N errors in one
    /// pass — guards against accidental short-circuiting in Phase 2.
    #[test]
    fn collects_multiple_defects_in_one_pass() {
        // Four defects: name contains a space, non-SemVer version, unknown
        // trigger, ftp URL.
        let input = r#"
manifest_schema_version = "1.0"

[plugin]
name = "Bad Name"
version = "1.2"
description = "multi-defect fixture"
triggers = ["on_startup"]
homepage = "ftp://bad"

[dependencies]
database_version = ">=3.0.0"
"#;
        let errors = Manifest::parse_toml(input).expect_err("should fail");
        let e = errors.errors();
        assert_eq!(
            e.len(),
            4,
            "expected 4 errors, got {}: {:?}",
            e.len(),
            e.iter().map(|r| &r.error).collect::<Vec<_>>()
        );

        let paths: Vec<&str> = e.iter().map(|r| r.path.as_str()).collect();
        assert!(
            paths.contains(&"plugin.name"),
            "missing plugin.name: {paths:?}"
        );
        assert!(
            paths.contains(&"plugin.version"),
            "missing plugin.version: {paths:?}"
        );
        assert!(
            paths.contains(&"plugin.triggers[0]"),
            "missing plugin.triggers[0]: {paths:?}"
        );
        assert!(
            paths.contains(&"plugin.homepage"),
            "missing plugin.homepage: {paths:?}"
        );
    }

    /// An unsupported major short-circuits before field-level validation,
    /// returning exactly 1 error even when other defects exist.
    #[test]
    fn schema_version_mismatch_short_circuits_with_single_error() {
        let input = r#"
manifest_schema_version = "99.0"

[plugin]
name = "Bad Name"
version = "1.0.0"
description = "x"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#;
        let errors = Manifest::parse_toml(input).expect_err("should fail");
        assert_eq!(
            errors.errors().len(),
            1,
            "short-circuit: expected exactly 1 error"
        );
        assert_matches::assert_matches!(
            errors.errors()[0].error,
            SchemaError::UnsupportedManifestMajor { .. }
        );
    }

    #[test]
    fn accepts_missing_exclude_defaults_empty() {
        let m = Manifest::parse_toml(MINIMAL).unwrap();
        assert!(m.plugin.exclude.is_empty());
    }

    #[test]
    fn accepts_empty_exclude() {
        let src = MINIMAL.replace(
            r#"triggers = ["process_writes"]"#,
            "triggers = [\"process_writes\"]\nexclude = []",
        );
        let m = Manifest::parse_toml(&src).unwrap();
        assert!(m.plugin.exclude.is_empty());
    }

    #[test]
    fn accepts_exclude_patterns_verbatim() {
        let src = MINIMAL.replace(
            r#"triggers = ["process_writes"]"#,
            "triggers = [\"process_writes\"]\nexclude = [\"tests/**\", \"*.pyc\"]",
        );
        let m = Manifest::parse_toml(&src).unwrap();
        assert_eq!(m.plugin.exclude, vec!["tests/**".to_string(), "*.pyc".to_string()]);
    }

    #[test]
    fn exclude_works_regardless_of_minor_version() {
        // Parser must not branch exclude support on the minor version.
        for ver in ["1.0", "1.1"] {
            let src = MINIMAL
                .replace(
                    r#"manifest_schema_version = "1.0""#,
                    &format!("manifest_schema_version = \"{ver}\""),
                )
                .replace(
                    r#"triggers = ["process_writes"]"#,
                    "triggers = [\"process_writes\"]\nexclude = [\"tests/**\"]",
                );
            let m = Manifest::parse_toml(&src).unwrap_or_else(|e| panic!("ver {ver}: {e}"));
            assert_eq!(m.plugin.exclude, vec!["tests/**".to_string()], "ver {ver}");
        }
    }

    #[test]
    fn rejects_non_array_exclude() {
        let src = MINIMAL.replace(
            r#"triggers = ["process_writes"]"#,
            "triggers = [\"process_writes\"]\nexclude = \"tests\"",
        );
        let errs = Manifest::parse_toml(&src).unwrap_err();
        assert_matches!(errs.errors()[0].error, SchemaError::TomlParse { .. });
    }

    #[test]
    fn rejects_non_string_exclude_item() {
        let src = MINIMAL.replace(
            r#"triggers = ["process_writes"]"#,
            "triggers = [\"process_writes\"]\nexclude = [1, 2]",
        );
        let errs = Manifest::parse_toml(&src).unwrap_err();
        assert_matches!(errs.errors()[0].error, SchemaError::TomlParse { .. });
    }

    /// A triple-quoted TOML string with embedded newlines must be rejected
    /// for `plugin.description`. (TOML strips the leading newline immediately
    /// after `"""`, so the rejection here fires on the inner `\n`s.)
    #[test]
    fn rejects_description_with_embedded_newline_in_toml() {
        let input = r#"
manifest_schema_version = "1.0"

[plugin]
name = "downsampler"
version = "1.2.0"
description = """
line one
line two
"""
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#;
        let errors = Manifest::parse_toml(input).expect_err("multiline description must fail");
        assert_eq!(errors.errors().len(), 1);
        let e = &errors.errors()[0];
        assert_eq!(e.path.as_str(), "plugin.description");
        assert_matches!(e.error, SchemaError::DescriptionMultiline { .. });
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
        let errors = Manifest::parse_toml(&manifest).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidUrlScheme { .. }
        );
        assert_eq!(errors.errors()[0].path.as_str(), &format!("plugin.{field}"));
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
        let errors = Manifest::parse_toml(input).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(errors.errors()[0].error, SchemaError::EmptyTriggers);
        assert_eq!(errors.errors()[0].path.as_str(), "plugin.triggers");
    }

    /// Invalid `dependencies.database_version` surfaces as
    /// `InvalidDatabaseVersion` with the `dependencies.database_version`
    /// path, not flattened through `serde::Error::custom`.
    #[test]
    fn rejects_invalid_database_version() {
        let input = r#"
manifest_schema_version = "1.0"

[plugin]
name = "x"
version = "1.0.0"
description = "x"
triggers = ["process_writes"]

[dependencies]
database_version = ">=not-a-version"
"#;
        let errors = Manifest::parse_toml(input).unwrap_err();
        assert_eq!(errors.errors().len(), 1);
        assert_matches!(
            errors.errors()[0].error,
            SchemaError::InvalidDatabaseVersion { .. }
        );
        assert_eq!(
            errors.errors()[0].path.as_str(),
            "dependencies.database_version"
        );
    }
}

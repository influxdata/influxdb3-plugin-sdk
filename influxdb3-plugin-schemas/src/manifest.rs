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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
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
        assert_matches!(
            Description::try_new(""),
            Err(SchemaError::DescriptionEmpty)
        );
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

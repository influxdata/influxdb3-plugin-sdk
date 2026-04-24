//! Plugin identity: `PluginId` tuple and `PluginName` newtype.

use crate::SchemaError;
use std::fmt;
use std::str::FromStr;

/// Validated plugin name matching `[a-z0-9][a-z0-9-]{0,63}` (1-64 chars,
/// starting with a lowercase alphanumeric, then lowercase alphanumerics or
/// hyphens).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginName(String);

impl PluginName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }

    fn validate(name: &str) -> Result<(), SchemaError> {
        let invalid = || SchemaError::InvalidPluginName {
            name: name.to_owned(),
        };
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 {
            return Err(invalid());
        }
        let first = bytes[0];
        let is_alnum_lower = |b: u8| b.is_ascii_digit() || b.is_ascii_lowercase();
        if !is_alnum_lower(first) {
            return Err(invalid());
        }
        for &b in &bytes[1..] {
            if !(is_alnum_lower(b) || b == b'-') {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

impl FromStr for PluginName {
    type Err = SchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::validate(s)?;
        Ok(Self(s.to_owned()))
    }
}

impl fmt::Display for PluginName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for PluginName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for PluginName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

/// Global plugin identity: the tuple `(source, name, version)` where `source`
/// is either a registry URL or a local directory. Two `PluginId`s are equal
/// when all three components match.
///
/// No serde impls: the SDK itself doesn't need them (manifests and indexes
/// use their own types). Add later with a snapshot test pinning the JSON shape
/// if a downstream consumer needs to persist `PluginId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginId {
    Registry {
        index_url: url::Url,
        name: PluginName,
        version: semver::Version,
    },
    Local {
        path: std::path::PathBuf,
        name: PluginName,
        version: semver::Version,
    },
}

impl PluginId {
    pub fn registry(index_url: url::Url, name: PluginName, version: semver::Version) -> Self {
        Self::Registry {
            index_url,
            name,
            version,
        }
    }

    pub fn local(path: std::path::PathBuf, name: PluginName, version: semver::Version) -> Self {
        Self::Local {
            path,
            name,
            version,
        }
    }

    pub fn name(&self) -> &PluginName {
        match self {
            Self::Registry { name, .. } | Self::Local { name, .. } => name,
        }
    }

    pub fn version(&self) -> &semver::Version {
        match self {
            Self::Registry { version, .. } | Self::Local { version, .. } => version,
        }
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry {
                index_url,
                name,
                version,
            } => write!(f, "{name}@{version} ({index_url})"),
            Self::Local {
                path,
                name,
                version,
            } => write!(f, "{name}@{version} (local: {})", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use rstest::rstest;

    #[rstest]
    #[case("a")]
    #[case("aa")]
    #[case("plugin")]
    #[case("my-plugin")]
    #[case("123")]
    #[case("a1b2c3")]
    #[case("a-really-long-but-still-valid-name-that-is-under-64-chars")]
    fn valid_names_accepted(#[case] input: &str) {
        let name = PluginName::from_str(input).expect("should accept valid name");
        assert_eq!(name.as_str(), input);
    }

    #[rstest]
    #[case("")] // empty
    #[case("-foo")] // leading hyphen
    #[case("Foo")] // uppercase
    #[case("foo_bar")] // underscore
    #[case("foo bar")] // space
    fn invalid_names_rejected(#[case] input: &str) {
        let err = PluginName::from_str(input).expect_err("should reject");
        assert_matches!(err, SchemaError::InvalidPluginName { .. });
    }

    /// Length boundaries 1, 64, 65. Dedicated test because `#[rstest::case]`
    /// arguments must be const-evaluable.
    #[test]
    fn length_boundaries() {
        assert!(PluginName::from_str("a").is_ok());
        assert!(PluginName::from_str(&"a".repeat(64)).is_ok());
        assert!(PluginName::from_str(&"a".repeat(65)).is_err());
    }

    #[test]
    fn plugin_name_display_matches_as_str() {
        let name = PluginName::from_str("downsampler").unwrap();
        assert_eq!(format!("{name}"), "downsampler");
    }

    #[test]
    fn plugin_name_round_trips_through_serde_json() {
        let name = PluginName::from_str("my-plugin").unwrap();
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, "\"my-plugin\"");
        let back: PluginName = serde_json::from_str(&json).unwrap();
        assert_eq!(back, name);
    }

    #[test]
    fn plugin_name_deserialize_rejects_invalid() {
        let result: Result<PluginName, _> = serde_json::from_str("\"Bad_Name\"");
        let err = result.expect_err("should reject invalid name");
        // serde flattens through `Deserialize`'s custom impl; the error message
        // must contain the normalization hint so consumers can understand what
        // went wrong. The exact prefix ("invalid plugin name") is pinned by
        // the SchemaError::InvalidPluginName Display snapshot in src/error.rs.
        assert!(
            err.to_string().contains("plugin name"),
            "expected error mentioning plugin name, got: {err}"
        );
    }
}

#[cfg(test)]
mod plugin_id_tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use semver::Version;
    use std::path::PathBuf;
    use url::Url;

    #[test]
    fn registry_variant_constructs_from_parts() {
        let id = PluginId::registry(
            Url::parse("https://plugins.example.com/index.json").unwrap(),
            PluginName::from_str("downsampler").unwrap(),
            Version::new(1, 2, 0),
        );
        match &id {
            PluginId::Registry { name, version, .. } => {
                assert_eq!(name.as_str(), "downsampler");
                assert_eq!(*version, Version::new(1, 2, 0));
            }
            PluginId::Local { .. } => panic!("expected Registry variant"),
        }
        assert_eq!(id.name().as_str(), "downsampler");
        assert_eq!(*id.version(), Version::new(1, 2, 0));
    }

    #[test]
    fn local_variant_constructs_from_parts() {
        let id = PluginId::local(
            PathBuf::from("/srv/plugins/my-plugin"),
            PluginName::from_str("my-plugin").unwrap(),
            Version::new(0, 3, 1),
        );
        match &id {
            PluginId::Local {
                path,
                name,
                version,
            } => {
                assert_eq!(*path, PathBuf::from("/srv/plugins/my-plugin"));
                assert_eq!(name.as_str(), "my-plugin");
                assert_eq!(*version, Version::new(0, 3, 1));
            }
            PluginId::Registry { .. } => panic!("expected Local variant"),
        }
        assert_eq!(id.name().as_str(), "my-plugin");
        assert_eq!(*id.version(), Version::new(0, 3, 1));
    }

    #[test]
    fn display_shape_pinned() {
        let registry = PluginId::registry(
            Url::parse("https://r.example/index.json").unwrap(),
            PluginName::from_str("downsampler").unwrap(),
            Version::new(1, 2, 0),
        );
        let local = PluginId::local(
            PathBuf::from("/srv/plugins/my-plugin"),
            PluginName::from_str("my-plugin").unwrap(),
            Version::new(0, 3, 1),
        );
        insta::assert_yaml_snapshot!(
            "plugin_id_display",
            vec![registry.to_string(), local.to_string()]
        );
    }
}

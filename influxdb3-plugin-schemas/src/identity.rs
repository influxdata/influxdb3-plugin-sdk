//! Plugin identity: `PluginId` tuple and `PluginName` newtype.

use crate::SchemaError;
use std::fmt;
use std::str::FromStr;

/// Validated plugin name. Conforms to the regex `[a-z0-9][a-z0-9-]{0,63}`
/// (1 to 64 characters, starting with lowercase alphanumeric, containing only
/// lowercase alphanumerics and hyphens).
///
/// Spec 1 defines this as the plugin identity's second element within a
/// registry.
///
/// # Regex divergence note
///
/// Spec 1 writes the regex as `[a-z0-9][a-z0-9-]{1,63}` (first char + 1..=63
/// more, total 2..=64). This implementation uses `{0,63}` (total 1..=64) so
/// length-1 names ("a") are accepted. The divergence is intentional and
/// matches the testing spec's S2 #2 boundary cases. Tracked in the plan's
/// "Design decisions locked here" section.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginName(String);

impl PluginName {
    /// Returns the underlying string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the newtype and returns the owned string.
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

/// Global plugin identity.
///
/// Per Spec 1 S1-5, plugin identity is the tuple `(source, name, version)`
/// where `source` is either a registry URL (`Registry` variant) or a local
/// directory path (`Local` variant). Two `PluginId`s compare equal when all
/// three components match.
///
/// # Serde impls deferred
///
/// `PluginId` intentionally does not derive `Serialize`/`Deserialize`. The
/// SDK itself does not need them — manifests and indexes use their own
/// types. If a downstream consumer (e.g., the database lockfile in Spec 4)
/// needs to persist `PluginId`, serde impls should be added at that point
/// under a dedicated task, with a snapshot test pinning the JSON shape.
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
    /// Constructs a registry-sourced `PluginId`.
    pub fn registry(index_url: url::Url, name: PluginName, version: semver::Version) -> Self {
        Self::Registry {
            index_url,
            name,
            version,
        }
    }

    /// Constructs a locally-sourced `PluginId`.
    pub fn local(path: std::path::PathBuf, name: PluginName, version: semver::Version) -> Self {
        Self::Local {
            path,
            name,
            version,
        }
    }

    /// Returns the plugin's name regardless of source.
    pub fn name(&self) -> &PluginName {
        match self {
            Self::Registry { name, .. } | Self::Local { name, .. } => name,
        }
    }

    /// Returns the plugin's version regardless of source.
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

    /// Length boundaries cannot be expressed as `#[rstest::case]` arguments
    /// (those require const-evaluable expressions), so they live in a
    /// dedicated test. Covers length 1 (valid), 64 (valid), 65 (rejected) —
    /// the "1-64 chars" contract from the plan's regex-divergence note.
    #[test]
    fn length_boundaries() {
        assert!(PluginName::from_str("a").is_ok()); // length 1
        assert!(PluginName::from_str(&"a".repeat(64)).is_ok()); // length 64
        assert!(PluginName::from_str(&"a".repeat(65)).is_err()); // length 65 rejected
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
        assert!(result.is_err());
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
    fn same_tuple_means_equal() {
        let a = PluginId::registry(
            Url::parse("https://r.example/index.json").unwrap(),
            PluginName::from_str("x").unwrap(),
            Version::new(1, 0, 0),
        );
        let b = PluginId::registry(
            Url::parse("https://r.example/index.json").unwrap(),
            PluginName::from_str("x").unwrap(),
            Version::new(1, 0, 0),
        );
        assert_eq!(a, b);
    }

    #[test]
    fn different_sources_are_not_equal() {
        let reg = PluginId::registry(
            Url::parse("https://r.example/index.json").unwrap(),
            PluginName::from_str("x").unwrap(),
            Version::new(1, 0, 0),
        );
        let local = PluginId::local(
            PathBuf::from("/tmp/x"),
            PluginName::from_str("x").unwrap(),
            Version::new(1, 0, 0),
        );
        assert_ne!(reg, local);
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

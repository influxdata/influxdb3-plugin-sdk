//! Plugin identity: `PluginId` tuple and `PluginName` newtype.

use crate::SchemaError;
use std::fmt;
use std::str::FromStr;

/// Validated plugin name matching `[a-zA-Z][a-zA-Z0-9_-]*` (1-64 ASCII
/// characters, starting with an ASCII letter). Case-preserving in storage.
/// Windows reserved device names (`con`, `prn`, `aux`, `nul`, `com0-9`,
/// `lpt0-9`) are rejected case-insensitively. Collisions inside a single
/// index use the [canonical form](Self::canonical).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginName(String);

impl PluginName {
    /// Windows reserved device names. Rejected case-insensitively because
    /// plugins extract to `plugin_dir/<name>/<version>/` and these names
    /// cannot be created as filesystem entries on Windows regardless of
    /// extension.
    const WINDOWS_RESERVED: &'static [&'static str] = &[
        "con", "prn", "aux", "nul",
        "com0", "com1", "com2", "com3", "com4",
        "com5", "com6", "com7", "com8", "com9",
        "lpt0", "lpt1", "lpt2", "lpt3", "lpt4",
        "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Canonical form for collision detection. Never surface to users;
    /// use `as_str()`/`Display` for presentation.
    ///
    /// Returns owned `String` rather than `Cow<str>`: the result differs
    /// from `as_str()` whenever the name contains any uppercase character
    /// or hyphen (common). Collision checks run O(n) in index size
    /// (v1 cap: ~200 plugins), so allocation cost is not load-bearing
    /// and the simpler type wins.
    pub fn canonical(&self) -> String {
        canonical_name(&self.0)
    }

    pub fn into_inner(self) -> String {
        self.0
    }

    fn validate(name: &str) -> Result<(), SchemaError> {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 {
            return Err(SchemaError::InvalidPluginName {
                name: name.to_owned(),
            });
        }
        let first = bytes[0];
        let is_alpha = |b: u8| b.is_ascii_uppercase() || b.is_ascii_lowercase();
        let is_alnum = |b: u8| b.is_ascii_digit() || is_alpha(b);
        if !is_alpha(first) {
            return Err(SchemaError::InvalidPluginName {
                name: name.to_owned(),
            });
        }
        for &b in &bytes[1..] {
            if !(is_alnum(b) || b == b'-' || b == b'_') {
                return Err(SchemaError::InvalidPluginName {
                    name: name.to_owned(),
                });
            }
        }
        let lower = name.to_ascii_lowercase();
        if Self::WINDOWS_RESERVED.iter().any(|&r| r == lower) {
            return Err(SchemaError::ReservedPluginName {
                name: name.to_owned(),
            });
        }
        Ok(())
    }
}

/// Canonical form used for `(name, version)` collision detection inside
/// a single index. Lives alongside `PluginName::canonical()` so the rule
/// is single-sourced; callers with a raw (un-validated) `&str` — notably
/// [`Index::from_raw_json`] — can dedupe without routing through the
/// validator.
pub(crate) fn canonical_name(raw: &str) -> String {
    raw.to_ascii_lowercase().replace('-', "_")
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
    #[case("a1b2c3")]
    #[case("a-really-long-but-still-valid-name-that-is-under-64-chars")]
    #[case("Z")]
    #[case("MyPlugin")]
    #[case("MYPLUGIN")]
    #[case("Test-1_v2")]
    #[case("Foo")]
    #[case("foo_bar")]
    fn valid_names_accepted(#[case] input: &str) {
        let name = PluginName::from_str(input).expect("should accept valid name");
        assert_eq!(name.as_str(), input);
    }

    #[rstest]
    #[case("")] // empty
    #[case("-foo")] // leading hyphen
    #[case("foo bar")] // space
    #[case("123")] // digit-leading
    #[case("7plugin")] // digit-leading
    #[case("café")] // non-ASCII
    #[case("foo.bar")] // dot
    fn invalid_names_rejected(#[case] input: &str) {
        let err = PluginName::from_str(input).expect_err("should reject");
        assert_matches!(err, SchemaError::InvalidPluginName { .. });
    }

    #[test]
    fn plugin_name_length_boundaries() {
        assert!(PluginName::from_str("a").is_ok());
        assert!(PluginName::from_str(&"a".repeat(64)).is_ok());
        assert!(matches!(
            PluginName::from_str(&"a".repeat(65)),
            Err(SchemaError::InvalidPluginName { .. })
        ));
        assert!(matches!(
            PluginName::from_str(""),
            Err(SchemaError::InvalidPluginName { .. })
        ));
    }

    #[rstest]
    #[case("con")]
    #[case("prn")]
    #[case("aux")]
    #[case("nul")]
    #[case("com0")]
    #[case("com9")]
    #[case("lpt0")]
    #[case("lpt9")]
    #[case("CON")]
    #[case("Com1")]
    fn reserved_names_rejected(#[case] input: &str) {
        let err = PluginName::from_str(input).expect_err("should reject reserved name");
        assert!(
            matches!(err, SchemaError::ReservedPluginName { ref name } if name == input),
            "expected ReservedPluginName with preserved input spelling, got: {err:?}"
        );
    }

    #[rstest]
    #[case("console")]
    #[case("com10")]
    #[case("conin")]
    #[case("com")]
    fn near_reserved_names_accepted(#[case] input: &str) {
        assert!(PluginName::from_str(input).is_ok());
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
        let result: Result<PluginName, _> = serde_json::from_str("\"Bad Name\"");
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

    #[rstest]
    #[case("a", "a")]
    #[case("MyPlugin", "myplugin")]
    #[case("foo-bar", "foo_bar")]
    #[case("foo_bar", "foo_bar")]
    #[case("Foo-Bar_Baz", "foo_bar_baz")]
    #[case("Test-1_v2", "test_1_v2")]
    fn canonical_form_matches_table(#[case] input: &str, #[case] expected: &str) {
        let name = PluginName::from_str(input).expect("valid input");
        assert_eq!(name.canonical(), expected);
        // Non-mutation invariant: canonical() does not change stored form
        assert_eq!(name.as_str(), input);
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

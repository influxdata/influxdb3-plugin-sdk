//! Field paths for error location within parsed documents.

use std::fmt;

/// A dotted-and-indexed path identifying a field inside a parsed manifest or
/// index (e.g., `plugin.name`, `plugins[3].dependencies.python[0]`). Build by
/// chaining from `FieldPath::root()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldPath(String);

impl FieldPath {
    pub fn root() -> Self {
        Self(String::new())
    }

    /// Appends `.name`, or just `name` when self is root.
    pub fn field(&self, name: &str) -> Self {
        if self.0.is_empty() {
            Self(name.to_owned())
        } else {
            Self(format!("{}.{}", self.0, name))
        }
    }

    /// Appends `[i]`.
    pub fn index(&self, i: usize) -> Self {
        Self(format!("{}[{i}]", self.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_empty() {
        assert_eq!(FieldPath::root().as_str(), "");
    }

    #[test]
    fn single_field() {
        assert_eq!(FieldPath::root().field("plugin").as_str(), "plugin");
    }

    #[test]
    fn nested_field() {
        assert_eq!(
            FieldPath::root().field("plugin").field("name").as_str(),
            "plugin.name"
        );
    }

    #[test]
    fn indexed_field() {
        assert_eq!(
            FieldPath::root()
                .field("plugins")
                .index(3)
                .field("hash")
                .as_str(),
            "plugins[3].hash"
        );
    }

    #[test]
    fn deep_path() {
        let p = FieldPath::root()
            .field("plugins")
            .index(3)
            .field("dependencies")
            .field("python")
            .index(0);
        assert_eq!(p.as_str(), "plugins[3].dependencies.python[0]");
    }

    #[test]
    fn display_matches_as_str() {
        let p = FieldPath::root().field("plugin").field("name");
        assert_eq!(format!("{p}"), "plugin.name");
    }
}

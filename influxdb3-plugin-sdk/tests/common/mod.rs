//! Shared test fixtures for integration tests in this crate.
//!
//! Integration-test files in Cargo are each their own crate, so a `mod common;`
//! declaration in each test file pulls this module in. Declare `#[allow(dead_code)]`
//! at the top — not every integration test uses every helper, and the compiler
//! would otherwise warn per-file.

#![allow(dead_code)]

use influxdb3_plugin_schemas::{ArtifactsUrl, Index, IndexSchemaVersion};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const VALID_MANIFEST: &str = r#"manifest_schema_version = "1.0"

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Test plugin."
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#;

pub(crate) const VALID_INIT: &str = "def process_writes(a, b, c):\n    pass\n";

/// Minimal plugin directory shape — manifest + __init__.py, nothing else.
/// Returns the path to the created directory.
pub(crate) fn minimal_plugin_dir(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("manifest.toml"), VALID_MANIFEST).unwrap();
    fs::write(dir.join("__init__.py"), VALID_INIT).unwrap();
    dir
}

pub(crate) fn empty_index() -> Index {
    Index {
        index_schema_version: IndexSchemaVersion::CURRENT,
        artifacts_url: ArtifactsUrl::try_new("https://plugins.example.com/artifacts").unwrap(),
        plugins: vec![],
    }
}

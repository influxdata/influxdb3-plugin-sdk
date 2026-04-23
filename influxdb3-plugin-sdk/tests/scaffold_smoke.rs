//! Integration tests for `sdk::scaffold::{plugin, registry}` at the external
//! test-crate layer. Complements the inline tests in `src/scaffold.rs` by
//! pinning the crate's pub-API boundary (signatures, re-export visibility,
//! error-type conversion).
//!
//! See `validate_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::{Index, Manifest, TriggerType};
use influxdb3_plugin_sdk::{SdkError, scaffold};
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "influxdb3-plugin-sdk-scaffold-smoke-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        Self(base)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn plugin_scaffold_produces_parseable_manifest() {
    let td = TempDir::new("plugin_parses");
    let dir = td.path().join("downsampler");
    scaffold::plugin(&dir, "downsampler", TriggerType::ProcessWrites)
        .expect("plugin scaffold should succeed");

    let manifest_raw = fs::read_to_string(dir.join("manifest.toml")).unwrap();
    let manifest = Manifest::parse_toml(&manifest_raw).expect("scaffolded manifest must parse");
    assert_eq!(manifest.plugin.name.as_str(), "downsampler");
    assert_eq!(manifest.plugin.triggers, vec![TriggerType::ProcessWrites]);

    assert!(dir.join("__init__.py").exists());
    assert!(dir.join("README.md").exists());
}

#[test]
fn plugin_scaffold_rejects_invalid_name() {
    let td = TempDir::new("plugin_bad_name");
    let dir = td.path().join("bad-name");
    let err = scaffold::plugin(&dir, "BAD_NAME", TriggerType::ProcessWrites).unwrap_err();
    assert!(matches!(
        err,
        SdkError::Schema(influxdb3_plugin_schemas::SchemaError::InvalidPluginName { .. })
    ));
}

#[test]
fn registry_scaffold_produces_parseable_index() {
    let td = TempDir::new("registry_parses");
    let dir = td.path().join("my-registry");
    scaffold::registry(&dir).expect("registry scaffold should succeed");

    let index_raw = fs::read_to_string(dir.join("index.json")).unwrap();
    let index = Index::parse_json(&index_raw).expect("scaffolded index must parse");
    assert!(index.plugins.is_empty());
    // artifacts_url validates as a file:// URL — the inline tests cover
    // this; the external test's contribution here is pinning the pub-API
    // re-export boundary.
    assert!(index.artifacts_url.to_string().starts_with("file://"));
}

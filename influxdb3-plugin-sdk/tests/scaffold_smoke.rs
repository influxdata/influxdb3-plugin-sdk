//! Integration tests for `sdk::scaffold::{plugin, registry}` at the external
//! test-crate layer. Complements the inline tests in `src/scaffold.rs` by
//! pinning the crate's pub-API boundary (signatures, re-export visibility,
//! error-type conversion).
//!
//! See `validate_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::{Index, Manifest, TriggerType};
use influxdb3_plugin_sdk::{SdkError, scaffold};
use semver as _;
use std::fs;

#[test]
fn plugin_scaffold_produces_parseable_manifest() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("downsampler");
    scaffold::plugin(&dir, "downsampler", TriggerType::ProcessWrites, None, false)
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
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("bad-name");
    let err =
        scaffold::plugin(&dir, "1bad", TriggerType::ProcessWrites, None, false).unwrap_err();
    assert!(matches!(
        err,
        SdkError::Schema(influxdb3_plugin_schemas::SchemaError::InvalidPluginName { .. })
    ));
}

#[test]
fn registry_scaffold_produces_parseable_index() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("my-registry");
    scaffold::registry(&dir, None, false).expect("registry scaffold should succeed");

    let index_raw = fs::read_to_string(dir.join("index.json")).unwrap();
    let index = Index::parse_json(&index_raw).expect("scaffolded index must parse");
    assert!(index.plugins.is_empty());
    // artifacts_url validates as a file:// URL — the inline tests cover
    // this; the external test's contribution here is pinning the pub-API
    // re-export boundary.
    assert!(index.artifacts_url.to_string().starts_with("file://"));
}

/// Scaffolded manifests must bake in the current schema version (`1.1`).
/// The parser only checks the major, so a minor drift is cosmetic — but
/// visible to any author diffing the scaffold against the spec.
#[test]
fn plugin_scaffold_emits_current_manifest_schema_version() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("downsampler");
    scaffold::plugin(&dir, "downsampler", TriggerType::ProcessWrites, None, false)
        .expect("plugin scaffold should succeed");

    let raw = fs::read_to_string(dir.join("manifest.toml")).unwrap();
    let manifest = Manifest::parse_toml(&raw).expect("scaffolded manifest must parse");
    assert_eq!(manifest.manifest_schema_version.major(), 1, "schema major");
    assert_eq!(
        manifest.manifest_schema_version.minor(),
        1,
        "schema minor: current = 1.1"
    );
}

#[test]
fn registry_scaffold_emits_current_index_schema_version() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("my-registry");
    scaffold::registry(&dir, None, false).expect("registry scaffold should succeed");

    let raw = fs::read_to_string(dir.join("index.json")).unwrap();
    let index = Index::parse_json(&raw).expect("scaffolded index must parse");
    assert_eq!(index.index_schema_version.major(), 1, "schema major");
    assert_eq!(
        index.index_schema_version.minor(),
        1,
        "schema minor: current = 1.1"
    );
}

/// `DEFAULT_DATABASE_VERSION` has the shape `>=<valid version>`. Catches a
/// refactor that drops the `>=` prefix (which `build.rs`'s
/// `VersionReq::parse` probe would accept as a bare version like `3.0.0`
/// but which violates the consumer contract that the scaffolded default
/// is a floor range).
#[test]
fn default_database_version_has_ge_prefix_over_valid_version() {
    const PREFIX: &str = ">=";
    let raw = scaffold::DEFAULT_DATABASE_VERSION;
    assert!(
        raw.starts_with(PREFIX),
        "DEFAULT_DATABASE_VERSION must start with `{PREFIX}`; got {raw:?}"
    );
    let remainder = &raw[PREFIX.len()..];
    assert!(
        semver::Version::parse(remainder).is_ok(),
        "DEFAULT_DATABASE_VERSION remainder {remainder:?} must parse as semver::Version"
    );
}

/// When `INFLUXDB3_PLUGIN_SDK_KNOWN_LATEST_DB` is unset at *build* time,
/// the scaffold default must be `>=3.0.0`. Gated on `sdk_known_db_is_fallback`,
/// which `build.rs` sets only when the fallback branch fires — so build-time
/// and test-time state can't disagree.
#[cfg(sdk_known_db_is_fallback)]
#[test]
fn default_database_version_fallback_is_3_0_0() {
    assert_eq!(scaffold::DEFAULT_DATABASE_VERSION, ">=3.0.0");
}

/// Pub-API boundary version of the inline scaffold test: a `ftp://` URL
/// must be rejected with `SdkError::Schema(UnsupportedArtifactScheme)`.
#[test]
fn registry_scaffold_rejects_ftp_url_at_api_boundary() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("reg");
    let err = scaffold::registry(&dir, Some("ftp://example.com/artifacts"), false).unwrap_err();
    assert!(matches!(
        err,
        SdkError::Schema(influxdb3_plugin_schemas::SchemaError::UnsupportedArtifactScheme { .. })
    ));
    assert!(!dir.join("index.json").exists());
}

/// Pub-API boundary version of the inline scaffold test: an unparseable
/// `--database-version` must be rejected with
/// `SdkError::Schema(InvalidDatabaseVersion)`.
#[test]
fn plugin_scaffold_rejects_invalid_database_version_at_api_boundary() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    let err = scaffold::plugin(
        &dir,
        "p",
        TriggerType::ProcessWrites,
        Some("not-a-range"),
        false,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        SdkError::Schema(influxdb3_plugin_schemas::SchemaError::InvalidDatabaseVersion { .. })
    ));
    assert!(!dir.join("manifest.toml").exists());
}

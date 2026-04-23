//! End-to-end smoke test for `sdk::package::package_plugin` — the
//! author-side pipeline composing validate → archive → hash → mutate_index.
//!
//! Covers the S2-2 happy path (append to a fresh index), the
//! `(name, version)` immutability check, and a round-trip through
//! `Index::to_canonical_json` → `Index::parse_json` to verify the derived
//! index is well-formed.
//!
//! See `validate_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::{ArtifactsUrl, Index, IndexSchemaVersion, TriggerType};
use influxdb3_plugin_sdk::package::package_plugin;
use influxdb3_plugin_sdk::scaffold;
use std::fs;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn empty_index() -> Index {
    Index {
        index_schema_version: IndexSchemaVersion::new(1, 0),
        artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
        plugins: vec![],
    }
}

#[test]
fn happy_path_against_valid_fixture() {
    let plugin_dir = fixtures().join("valid_plugin");
    let out = package_plugin(&plugin_dir, empty_index()).expect("package should succeed");

    // Archive has bytes; hash format is canonical.
    assert!(!out.archive_bytes.is_empty());
    assert!(out.hash.as_str().starts_with("sha256:"));
    assert_eq!(out.hash.as_str().len(), "sha256:".len() + 64);

    // Derived index now carries exactly one entry.
    assert_eq!(out.derived_index.plugins.len(), 1);
    let entry = &out.derived_index.plugins[0];
    assert_eq!(entry.name.as_str(), "valid-plugin");
    assert_eq!(entry.version, semver::Version::new(0, 1, 0));
    assert_eq!(entry.hash, out.hash);
    assert!(!entry.yanked);
}

#[test]
fn derived_index_round_trips_through_canonical_json() {
    let plugin_dir = fixtures().join("valid_plugin");
    let out = package_plugin(&plugin_dir, empty_index()).unwrap();

    let json = out
        .derived_index
        .to_canonical_json()
        .expect("derived index should serialize");
    let reparsed = Index::parse_json(&json).expect("canonical JSON should re-parse");
    assert_eq!(reparsed.plugins.len(), 1);
    assert_eq!(reparsed.plugins[0].name.as_str(), "valid-plugin");
}

#[test]
fn immutability_check_rejects_duplicate_name_version() {
    use influxdb3_plugin_sdk::SdkError;

    let plugin_dir = fixtures().join("valid_plugin");
    let first = package_plugin(&plugin_dir, empty_index()).unwrap();

    let err = package_plugin(&plugin_dir, first.derived_index).unwrap_err();
    assert!(
        matches!(err, SdkError::AlreadyPublished { .. }),
        "expected AlreadyPublished, got {err:?}"
    );
}

#[test]
fn validation_failure_prevents_packaging() {
    use influxdb3_plugin_sdk::SdkError;

    let plugin_dir = fixtures().join("invalid_plugins/missing_trigger_impl");
    let err = package_plugin(&plugin_dir, empty_index()).unwrap_err();
    assert!(
        matches!(err, SdkError::ValidationErrors(_)),
        "expected ValidationErrors, got {err:?}"
    );
}

#[test]
fn archive_is_extractable_and_contains_expected_entries() {
    let plugin_dir = fixtures().join("valid_plugin");
    let out = package_plugin(&plugin_dir, empty_index()).unwrap();

    // Gunzip + parse tar listing to verify the archive is well-formed and
    // carries the two required files under the `{name}-{version}/` root.
    let tar_bytes = {
        use flate2::read::GzDecoder;
        let mut decoder = GzDecoder::new(out.archive_bytes.as_slice());
        let mut buf = Vec::new();
        std::io::copy(&mut decoder, &mut buf).unwrap();
        buf
    };
    let mut archive = tar::Archive::new(std::io::Cursor::new(tar_bytes));
    let paths: Vec<String> = archive
        .entries_with_seek()
        .unwrap()
        .filter_map(|e| {
            e.ok()
                .and_then(|e| e.header().path().ok().map(|p| p.display().to_string()))
        })
        .collect();
    assert!(paths.contains(&"valid-plugin-0.1.0/manifest.toml".to_owned()));
    assert!(paths.contains(&"valid-plugin-0.1.0/__init__.py".to_owned()));
}

/// End-to-end canary: the scaffold template's output must itself be
/// valid and packageable. Catches "template produces something that won't
/// validate" regressions — a bug class that single-layer tests miss.
/// Testing-spec Section 5 #12.
#[test]
fn scaffold_then_validate_then_package_round_trips() {
    let td_root = std::env::temp_dir().join(format!(
        "influxdb3-plugin-sdk-scaffold-to-package-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&td_root);
    fs::create_dir_all(&td_root).unwrap();

    let plugin_dir = td_root.join("scaffolded");
    scaffold::plugin(&plugin_dir, "scaffolded", TriggerType::ProcessWrites)
        .expect("scaffold should succeed");

    let out =
        package_plugin(&plugin_dir, empty_index()).expect("scaffolded plugin must package cleanly");
    assert_eq!(out.new_entry.name.as_str(), "scaffolded");
    assert_eq!(out.new_entry.version, semver::Version::new(0, 1, 0));

    let _ = fs::remove_dir_all(&td_root);
}

// Verify the pipeline writes no files — the library layer owns bytes only.
#[test]
fn pipeline_writes_no_files_to_disk() {
    let plugin_dir = fixtures().join("valid_plugin");
    // Snapshot the mtime of the plugin dir pre-call; if package_plugin
    // somehow wrote inside it, mtime would shift.
    let before = fs::metadata(&plugin_dir).unwrap().modified().unwrap();
    let _ = package_plugin(&plugin_dir, empty_index()).unwrap();
    let after = fs::metadata(&plugin_dir).unwrap().modified().unwrap();
    assert_eq!(before, after);
}
